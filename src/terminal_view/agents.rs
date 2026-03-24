use super::*;
use alacritty_terminal::grid::Dimensions;
use gpui::{ObjectFit, StatefulInteractiveElement, StyledImage, img};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;

const AGENT_WORKSPACE_STATE_FILE: &str = "agents.json";
const AGENT_WORKSPACE_STATE_VERSION: u64 = 1;
const AGENT_SIDEBAR_WIDTH: f32 = 252.0;
const AGENT_SIDEBAR_HEADER_HEIGHT: f32 = 36.0;
const AGENT_SIDEBAR_PROJECT_ROW_HEIGHT: f32 = 30.0;
const AGENT_STATUS_VISIBLE_LINE_COUNT: i32 = 6;
static NEXT_AGENT_ENTITY_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgentThreadRuntimeStatus {
    Busy,
    Ready,
    Saved,
}

impl AgentThreadRuntimeStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Ready => "ready",
            Self::Saved => "saved",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgentThreadStatusTone {
    Active,
    Warning,
    Error,
    Muted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AgentThreadStatusPresentation {
    label: String,
    detail: Option<String>,
    tone: AgentThreadStatusTone,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedAgentWorkspaceState {
    version: u64,
    sidebar_open: bool,
    active_project_id: Option<String>,
    #[serde(default)]
    collapsed_project_ids: Vec<String>,
    projects: Vec<AgentProject>,
    threads: Vec<AgentThread>,
}

impl Default for PersistedAgentWorkspaceState {
    fn default() -> Self {
        Self {
            version: AGENT_WORKSPACE_STATE_VERSION,
            sidebar_open: false,
            active_project_id: None,
            collapsed_project_ids: Vec::new(),
            projects: Vec::new(),
            threads: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct AgentProject {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) root_path: String,
    pub(super) created_at_ms: u64,
    pub(super) updated_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct AgentThread {
    id: String,
    project_id: String,
    agent: command_palette::AiAgentPreset,
    title: String,
    #[serde(default)]
    custom_title: Option<String>,
    launch_command: String,
    working_dir: String,
    last_seen_title: Option<String>,
    last_seen_command: Option<String>,
    #[serde(default)]
    last_status_label: Option<String>,
    #[serde(default)]
    last_status_detail: Option<String>,
    created_at_ms: u64,
    updated_at_ms: u64,
    #[serde(skip)]
    linked_tab_id: Option<TabId>,
}

fn next_agent_entity_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let counter = NEXT_AGENT_ENTITY_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{millis}-{counter}")
}

fn now_unix_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

impl TerminalView {
    pub(in super::super) fn agent_sidebar_width(&self) -> f32 {
        if self.should_render_agent_sidebar() {
            AGENT_SIDEBAR_WIDTH
        } else {
            0.0
        }
    }

    pub(in super::super) fn terminal_left_sidebar_width(&self) -> f32 {
        self.tab_strip_sidebar_width() + self.agent_sidebar_width()
    }

    pub(super) fn should_render_agent_sidebar(&self) -> bool {
        self.agent_sidebar_enabled && self.agent_sidebar_open
    }

    fn persisted_agent_workspace_path() -> Result<PathBuf, String> {
        let config_path = crate::config::ensure_config_file().map_err(|error| error.to_string())?;
        let parent = config_path
            .parent()
            .ok_or_else(|| format!("Invalid config path '{}'", config_path.display()))?;
        Ok(parent.join(AGENT_WORKSPACE_STATE_FILE))
    }

    fn load_persisted_agent_workspace_state() -> Result<PersistedAgentWorkspaceState, String> {
        let path = Self::persisted_agent_workspace_path()?;
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PersistedAgentWorkspaceState::default());
            }
            Err(error) => {
                return Err(format!(
                    "Failed to read agent workspace state '{}': {}",
                    path.display(),
                    error
                ));
            }
        };

        let mut state: PersistedAgentWorkspaceState = serde_json::from_str(&contents)
            .map_err(|error| format!("Invalid agent workspace JSON: {}", error))?;
        if state.version != AGENT_WORKSPACE_STATE_VERSION {
            return Err(format!(
                "Unsupported agent workspace state version {}",
                state.version
            ));
        }
        for thread in &mut state.threads {
            thread.linked_tab_id = None;
        }
        Ok(state)
    }

    fn store_persisted_agent_workspace_state(&self) -> Result<(), String> {
        let path = Self::persisted_agent_workspace_path()?;
        let parent = path
            .parent()
            .ok_or_else(|| format!("Invalid agent workspace state path '{}'", path.display()))?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create '{}': {}", parent.display(), error))?;

        let state = PersistedAgentWorkspaceState {
            version: AGENT_WORKSPACE_STATE_VERSION,
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
        let contents = serde_json::to_string_pretty(&state)
            .map_err(|error| format!("Failed to encode agent workspace: {}", error))?;

        let mut temp = NamedTempFile::new_in(parent).map_err(|error| {
            format!(
                "Failed to create temp file in '{}': {}",
                parent.display(),
                error
            )
        })?;
        temp.write_all(contents.as_bytes())
            .map_err(|error| format!("Failed to write agent workspace: {}", error))?;
        temp.flush()
            .map_err(|error| format!("Failed to flush agent workspace: {}", error))?;
        temp.as_file()
            .sync_all()
            .map_err(|error| format!("Failed to sync agent workspace: {}", error))?;
        temp.persist(&path).map_err(|error| {
            format!(
                "Failed to persist agent workspace '{}': {}",
                path.display(),
                error.error
            )
        })?;
        Ok(())
    }

    pub(super) fn restore_persisted_agent_workspace(&mut self) {
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
                        .agent_projects
                        .iter()
                        .max_by_key(|project| project.updated_at_ms)
                        .map(|project| project.id.clone());
                }
            }
            Err(error) => {
                log::error!("Failed to restore agent workspace: {}", error);
                self.agent_sidebar_open = self.agent_sidebar_enabled;
            }
        }
    }

    pub(super) fn sync_persisted_agent_workspace(&self) {
        if let Err(error) = self.store_persisted_agent_workspace_state() {
            log::error!("Failed to persist agent workspace: {}", error);
        }
    }

    pub(super) fn toggle_agent_sidebar(&mut self, cx: &mut Context<Self>) {
        if !self.agent_sidebar_enabled {
            termy_toast::info(
                "Enable agent_sidebar_enabled in config.txt to use the agent workspace",
            );
            self.notify_overlay(cx);
            return;
        }

        self.agent_sidebar_open = !self.agent_sidebar_open;
        self.sync_persisted_agent_workspace();
        cx.notify();
    }

    pub(super) fn normalized_agent_working_dir(
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

    fn preferred_working_dir_for_new_agent_session(
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
            .and_then(|pid| {
                self.cached_or_queued_working_dir_for_child_pid(pid, cx)
                    .or_else(|| {
                        let value = Self::working_dir_for_child_pid_blocking(pid);
                        self.complete_child_working_dir_lookup(pid, value.clone());
                        value
                    })
            });
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

    fn agent_project_name_for_path(path: &str) -> String {
        let path_obj = Path::new(path);
        path_obj
            .file_name()
            .and_then(|segment| segment.to_str())
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| Self::display_working_directory_for_prompt(path_obj))
    }

    fn ensure_agent_project(&mut self, root_path: &str) -> String {
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
            created_at_ms: now,
            updated_at_ms: now,
        });
        project_id
    }

    fn touch_agent_project(&mut self, project_id: &str) -> Option<String> {
        let project = self
            .agent_projects
            .iter_mut()
            .find(|project| project.id == project_id)?;
        project.updated_at_ms = now_unix_ms();
        Some(project.root_path.clone())
    }

    fn create_agent_thread_for_active_tab(
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

    fn resume_saved_agent_thread(
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

    pub(super) fn launch_ai_agent_from_palette(
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

    fn detach_agent_thread_from_live_tab(&mut self, thread_id: &str) {
        let linked_tab_id = self
            .agent_threads
            .iter()
            .find(|thread| thread.id == thread_id)
            .and_then(|thread| thread.linked_tab_id);
        let Some(tab_id) = linked_tab_id else {
            return;
        };
        let Some(tab_index) = self.tab_index_by_id(tab_id) else {
            return;
        };
        if let Some(tab) = self.tabs.get_mut(tab_index)
            && tab.agent_thread_id.as_deref() == Some(thread_id)
        {
            tab.agent_thread_id = None;
        }
    }

    pub(super) fn delete_agent_thread(&mut self, thread_id: &str) -> Result<(), String> {
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

    pub(super) fn delete_agent_project(&mut self, project_id: &str) -> Result<usize, String> {
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

        if self.active_agent_project_id.as_deref() == Some(project_id) {
            self.active_agent_project_id = self
                .sorted_agent_projects()
                .first()
                .map(|project| project.id.clone());
        }

        self.sync_persisted_agent_workspace();
        Ok(removed_threads)
    }

    pub(super) fn agent_thread_archive_snapshot_for_tab(
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

    pub(super) fn archive_agent_thread_snapshot(
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

    pub(super) fn sync_agent_workspace_to_active_tab(&mut self) {
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

    fn thread_project_id(&self, thread_id: &str) -> Option<&str> {
        self.agent_threads
            .iter()
            .find(|thread| thread.id == thread_id)
            .map(|thread| thread.project_id.as_str())
    }

    pub(super) fn begin_rename_agent_thread(&mut self, thread_id: &str, cx: &mut Context<Self>) {
        let Some(initial_title) = self
            .agent_threads
            .iter()
            .find(|thread| thread.id == thread_id)
            .map(|thread| {
                thread
                    .custom_title
                    .clone()
                    .unwrap_or_else(|| self.agent_thread_display_title(thread))
            })
        else {
            return;
        };

        if self.is_command_palette_open() {
            self.close_command_palette(cx);
        }
        if self.search_open {
            self.close_search(cx);
        }

        self.renaming_agent_thread_id = Some(thread_id.to_string());
        self.agent_thread_rename_input.set_text(initial_title);
        self.reset_cursor_blink_phase();
        self.inline_input_selecting = false;
        cx.notify();
    }

    pub(super) fn commit_rename_agent_thread(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.renaming_agent_thread_id.clone() else {
            return;
        };
        let Some(thread) = self
            .agent_threads
            .iter_mut()
            .find(|thread| thread.id == thread_id)
        else {
            self.cancel_rename_agent_thread(cx);
            return;
        };

        let trimmed = self.agent_thread_rename_input.text().trim();
        thread.custom_title = (!trimmed.is_empty()).then(|| Self::truncate_tab_title(trimmed));
        thread.updated_at_ms = now_unix_ms();
        self.sync_persisted_agent_workspace();
        self.cancel_rename_agent_thread(cx);
    }

    pub(super) fn cancel_rename_agent_thread(&mut self, cx: &mut Context<Self>) {
        if self.renaming_agent_thread_id.take().is_some()
            || !self.agent_thread_rename_input.text().is_empty()
        {
            self.agent_thread_rename_input.clear();
            self.inline_input_selecting = false;
            cx.notify();
        }
    }

    fn toggle_agent_project_collapsed(&mut self, project_id: &str, cx: &mut Context<Self>) {
        if self.collapsed_agent_project_ids.contains(project_id) {
            self.collapsed_agent_project_ids.remove(project_id);
        } else {
            self.collapsed_agent_project_ids
                .insert(project_id.to_string());
            if self
                .renaming_agent_thread_id
                .as_deref()
                .and_then(|thread_id| self.thread_project_id(thread_id))
                == Some(project_id)
            {
                self.cancel_rename_agent_thread(cx);
            }
        }
        self.sync_persisted_agent_workspace();
        cx.notify();
    }

    fn confirm_agent_thread_delete(&self, thread: &AgentThread) -> bool {
        let thread_title = thread
            .last_seen_title
            .as_deref()
            .unwrap_or(thread.title.as_str());
        let message = if thread.linked_tab_id.is_some() {
            format!(
                "Delete the thread \"{}\" from the sidebar?\n\nThe terminal tab stays open, but it will no longer be tracked as an agent thread.",
                thread_title
            )
        } else {
            format!("Delete the saved thread \"{}\"?", thread_title)
        };
        termy_native_sdk::confirm("Delete Agent Thread", &message)
    }

    fn confirm_agent_project_delete(&self, project: &AgentProject, thread_count: usize) -> bool {
        let message = if thread_count == 0 {
            format!(
                "Delete the project \"{}\"?\n\nIts folder reference will be removed from the agent sidebar.",
                project.name
            )
        } else {
            format!(
                "Delete the project \"{}\" and its {} thread(s)?\n\nOpen terminal tabs stay open, but they will no longer be tracked in the agent sidebar.",
                project.name, thread_count
            )
        };
        termy_native_sdk::confirm("Delete Agent Project", &message)
    }

    pub(super) fn open_ai_agents_palette_for_project_from_sidebar(
        &mut self,
        project_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.command_palette.set_agent_launch_project_id(project_id);
        self.open_command_palette_in_mode(command_palette::CommandPaletteMode::Agents, cx);
    }

    fn schedule_agent_project_context_menu(&mut self, project_id: String, cx: &mut Context<Self>) {
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let action = smol::unblock(termy_native_sdk::show_agent_project_context_menu).await;
            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    let Some(action) = action else {
                        return;
                    };
                    match action {
                        termy_native_sdk::AgentProjectContextMenuAction::NewSession => {
                            view.open_ai_agents_palette_for_project_from_sidebar(
                                Some(project_id.clone()),
                                cx,
                            );
                        }
                        termy_native_sdk::AgentProjectContextMenuAction::DeleteProject => {
                            let Some(project) = view
                                .agent_projects
                                .iter()
                                .find(|project| project.id == project_id)
                                .cloned()
                            else {
                                return;
                            };
                            let thread_count = view.project_thread_count(project_id.as_str());
                            if !view.confirm_agent_project_delete(&project, thread_count) {
                                return;
                            }
                            match view.delete_agent_project(project_id.as_str()) {
                                Ok(_) => {
                                    termy_toast::success(format!(
                                        "Deleted project \"{}\"",
                                        project.name
                                    ));
                                    view.notify_overlay(cx);
                                    cx.notify();
                                }
                                Err(error) => {
                                    termy_toast::error(error);
                                    view.notify_overlay(cx);
                                }
                            }
                        }
                    }
                })
            });
        })
        .detach();
    }

    fn schedule_agent_thread_context_menu(&mut self, thread_id: String, cx: &mut Context<Self>) {
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let action = smol::unblock(termy_native_sdk::show_agent_thread_context_menu).await;
            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    let Some(action) = action else {
                        return;
                    };
                    match action {
                        termy_native_sdk::AgentThreadContextMenuAction::DeleteThread => {
                            let Some(thread) = view
                                .agent_threads
                                .iter()
                                .find(|thread| thread.id == thread_id)
                                .cloned()
                            else {
                                return;
                            };
                            if !view.confirm_agent_thread_delete(&thread) {
                                return;
                            }
                            match view.delete_agent_thread(thread_id.as_str()) {
                                Ok(()) => {
                                    termy_toast::success("Deleted agent thread");
                                    view.notify_overlay(cx);
                                    cx.notify();
                                }
                                Err(error) => {
                                    termy_toast::error(error);
                                    view.notify_overlay(cx);
                                }
                            }
                        }
                    }
                })
            });
        })
        .detach();
    }

    fn agent_thread_runtime_status(&self, thread: &AgentThread) -> AgentThreadRuntimeStatus {
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

    fn extract_agent_status_line(
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

    fn normalize_agent_status_line(line: &str) -> Option<String> {
        let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
        let normalized = normalized.trim();
        if normalized.is_empty() || !normalized.chars().any(|ch| ch.is_alphanumeric()) {
            return None;
        }
        Some(normalized.to_string())
    }

    fn collect_visible_agent_status_lines(terminal: &Terminal) -> Vec<String> {
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

    fn find_last_status_line(lines: &[String], needles: &[&str]) -> Option<String> {
        lines.iter().rev().find_map(|line| {
            let normalized = line.to_ascii_lowercase();
            needles
                .iter()
                .any(|needle| normalized.contains(needle))
                .then(|| line.clone())
        })
    }

    fn detect_generic_agent_status(lines: &[String]) -> Option<AgentThreadStatusPresentation> {
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

        None
    }

    fn detect_provider_status(
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
            command_palette::AiAgentPreset::OpenCode => None,
            command_palette::AiAgentPreset::Codex => None,
        }
    }

    fn live_agent_thread_fallback_detail(&self, thread: &AgentThread, tab: &TerminalTab) -> String {
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

    fn saved_agent_thread_detail(&self, thread: &AgentThread) -> Option<String> {
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

    fn agent_thread_status_presentation(
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

    fn agent_thread_display_title(&self, thread: &AgentThread) -> String {
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

    fn project_thread_count(&self, project_id: &str) -> usize {
        self.agent_threads
            .iter()
            .filter(|thread| thread.project_id == project_id)
            .count()
    }

    fn sorted_agent_projects(&self) -> Vec<&AgentProject> {
        let mut projects = self.agent_projects.iter().collect::<Vec<_>>();
        projects.sort_by_key(|project| std::cmp::Reverse(project.updated_at_ms));
        projects
    }

    fn sorted_agent_threads_for_project(&self, project_id: &str) -> Vec<&AgentThread> {
        let mut threads = self
            .agent_threads
            .iter()
            .filter(|thread| thread.project_id == project_id)
            .collect::<Vec<_>>();
        threads.sort_by_key(|thread| std::cmp::Reverse(thread.updated_at_ms));
        threads
    }

    fn agent_thread_relative_age(updated_at_ms: u64) -> String {
        let now = now_unix_ms();
        let elapsed_seconds = now
            .saturating_sub(updated_at_ms)
            .checked_div(1000)
            .unwrap_or_default();

        match elapsed_seconds {
            0..=59 => "now".to_string(),
            60..=3599 => format!("{}m", elapsed_seconds / 60),
            3600..=86_399 => format!("{}h", elapsed_seconds / 3600),
            86_400..=604_799 => format!("{}d", elapsed_seconds / 86_400),
            604_800..=2_592_000 => format!("{}w", elapsed_seconds / 604_800),
            _ => format!("{}mo", elapsed_seconds / 2_592_000),
        }
    }

    fn compact_agent_thread_detail(
        status: &AgentThreadStatusPresentation,
        is_active: bool,
    ) -> Option<String> {
        match status.tone {
            AgentThreadStatusTone::Error | AgentThreadStatusTone::Warning => {
                status.detail.clone().or_else(|| Some(status.label.clone()))
            }
            AgentThreadStatusTone::Active if is_active => match status.label.as_str() {
                "thinking" => Some("Thinking".to_string()),
                "tool" => Some("Using tools".to_string()),
                "approval" => Some("Approval required".to_string()),
                "starting" => Some("Starting".to_string()),
                _ => None,
            },
            AgentThreadStatusTone::Muted => None,
            AgentThreadStatusTone::Active => None,
        }
    }

    fn render_agent_project_glyph(stroke: gpui::Rgba, bg: gpui::Rgba) -> AnyElement {
        div()
            .relative()
            .flex_none()
            .w(px(15.0))
            .h(px(12.0))
            .child(
                div()
                    .absolute()
                    .left(px(1.0))
                    .top(px(1.0))
                    .w(px(5.0))
                    .h(px(3.0))
                    .bg(bg)
                    .border_1()
                    .border_color(stroke),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(3.0))
                    .w(px(14.0))
                    .h(px(8.0))
                    .bg(bg)
                    .border_1()
                    .border_color(stroke),
            )
            .into_any_element()
    }

    fn render_agent_sidebar_avatar(
        agent: command_palette::AiAgentPreset,
        dark_surface: bool,
        border: gpui::Rgba,
        bg: gpui::Rgba,
        text: gpui::Rgba,
    ) -> AnyElement {
        let fallback_label = agent.fallback_label();
        div()
            .flex_none()
            .size(px(16.0))
            .p(px(1.5))
            .bg(bg)
            .border_1()
            .border_color(border)
            .child(
                img(Path::new(agent.image_asset_path(dark_surface)))
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .with_fallback(move || {
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(7.0))
                            .text_color(text)
                            .child(fallback_label)
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

    fn render_agent_status_badge(
        label: &str,
        tone: AgentThreadStatusTone,
        border: gpui::Rgba,
        bg: gpui::Rgba,
        text: gpui::Rgba,
        muted: gpui::Rgba,
        warning: gpui::Rgba,
        error: gpui::Rgba,
    ) -> AnyElement {
        let badge_text = match tone {
            AgentThreadStatusTone::Active => text,
            AgentThreadStatusTone::Warning => warning,
            AgentThreadStatusTone::Error => error,
            AgentThreadStatusTone::Muted => muted,
        };

        div()
            .flex_none()
            .h(px(16.0))
            .px(px(5.0))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_size(px(9.5))
            .text_color(badge_text)
            .child(label.to_ascii_lowercase())
            .into_any_element()
    }

    fn render_agent_sidebar_new_session_icon(stroke: gpui::Rgba, bg: gpui::Rgba) -> AnyElement {
        div()
            .relative()
            .flex_none()
            .w(px(17.0))
            .h(px(14.0))
            .child(
                div()
                    .absolute()
                    .left(px(1.0))
                    .top(px(2.0))
                    .w(px(5.0))
                    .h(px(3.0))
                    .bg(bg)
                    .border_1()
                    .border_color(stroke),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(4.0))
                    .w(px(11.0))
                    .h(px(8.0))
                    .bg(bg)
                    .border_1()
                    .border_color(stroke),
            )
            .child(
                div()
                    .absolute()
                    .right(px(1.0))
                    .top(px(3.0))
                    .w(px(1.5))
                    .h(px(7.0))
                    .bg(stroke),
            )
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top(px(6.0))
                    .w(px(5.0))
                    .h(px(1.5))
                    .bg(stroke),
            )
            .into_any_element()
    }

    fn render_agent_sidebar_hide_icon(stroke: gpui::Rgba) -> AnyElement {
        div()
            .relative()
            .flex_none()
            .w(px(15.0))
            .h(px(12.0))
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top(px(1.0))
                    .w(px(12.0))
                    .h(px(1.5))
                    .bg(stroke),
            )
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top(px(5.0))
                    .w(px(9.0))
                    .h(px(1.5))
                    .bg(stroke),
            )
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top(px(9.0))
                    .w(px(6.0))
                    .h(px(1.5))
                    .bg(stroke),
            )
            .into_any_element()
    }

    pub(super) fn render_agent_sidebar(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.should_render_agent_sidebar() {
            return None;
        }

        let overlay_style = self.overlay_style();
        let panel_bg = overlay_style.chrome_panel_background_with_floor(0.96, 0.88);
        let input_bg = overlay_style.chrome_panel_background_with_floor(0.74, 0.72);
        let transparent = overlay_style.transparent_background();
        let text = overlay_style.panel_foreground(0.94);
        let muted = overlay_style.panel_foreground(0.62);
        let border = resolve_chrome_stroke_color(
            panel_bg,
            self.colors.foreground,
            self.chrome_contrast_profile().stroke_mix,
        );
        let selected_bg = overlay_style.panel_cursor(0.10);
        let button_hover_bg = overlay_style.chrome_panel_cursor(0.14);
        let dark_surface = command_palette::AiAgentPreset::prefers_light_asset_variant(panel_bg);
        let active_thread_id = self
            .tabs
            .get(self.active_tab)
            .and_then(|tab| tab.agent_thread_id.as_deref())
            .map(str::to_string);
        let project_groups = self
            .sorted_agent_projects()
            .into_iter()
            .enumerate()
            .map(|(index, project)| {
                let project_id = project.id.clone();
                let project_context_menu_id = project.id.clone();
                let is_project_active =
                    self.active_agent_project_id.as_deref() == Some(project_id.as_str());
                let is_collapsed = self
                    .collapsed_agent_project_ids
                    .contains(project_id.as_str());
                let project_threads = self.sorted_agent_threads_for_project(project_id.as_str());

                let project_row = div()
                    .id(SharedString::from(format!("agent-project-{}", project.id)))
                    .w_full()
                    .h(px(AGENT_SIDEBAR_PROJECT_ROW_HEIGHT))
                    .px(px(14.0))
                    .mt(px(if index == 0 { 6.0 } else { 12.0 }))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _event, _window, cx| {
                            let was_active = view.active_agent_project_id.as_deref()
                                == Some(project_id.as_str());
                            view.active_agent_project_id = Some(project_id.clone());
                            if was_active {
                                view.toggle_agent_project_collapsed(project_id.as_str(), cx);
                            } else {
                                view.collapsed_agent_project_ids.remove(project_id.as_str());
                                view.sync_persisted_agent_workspace();
                                cx.notify();
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |view, _event, _window, cx| {
                            view.schedule_agent_project_context_menu(
                                project_context_menu_id.clone(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        div()
                            .w(px(8.0))
                            .flex_none()
                            .text_size(px(9.0))
                            .text_color(muted)
                            .child(if is_collapsed { ">" } else { "v" }),
                    )
                    .child(Self::render_agent_project_glyph(
                        if is_project_active { text } else { muted },
                        panel_bg,
                    ))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(13.0))
                            .text_color(if is_project_active { text } else { muted })
                            .child(project.name.clone()),
                    )
                    .into_any_element();

                let thread_rows = (!is_collapsed)
                    .then_some(project_threads)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|thread| {
                        let thread_id = thread.id.clone();
                        let thread_context_menu_id = thread.id.clone();
                        let is_renaming_thread =
                            self.renaming_agent_thread_id.as_deref() == Some(thread_id.as_str());
                        let status = self.agent_thread_status_presentation(thread);
                        let is_active = active_thread_id.as_deref() == Some(thread_id.as_str());
                        let title = self.agent_thread_display_title(thread);
                        let age = Self::agent_thread_relative_age(thread.updated_at_ms);
                        let detail = (!is_renaming_thread)
                            .then(|| {
                                Self::compact_agent_thread_detail(&status, is_active)
                                    .or_else(|| status.detail.clone())
                            })
                            .flatten();
                        let linked_tab_id = thread.linked_tab_id;

                        div()
                            .id(SharedString::from(format!("agent-thread-{}", thread.id)))
                            .w_full()
                            .px(px(14.0))
                            .py(px(if detail.is_some() || is_renaming_thread {
                                5.0
                            } else {
                                7.0
                            }))
                            .rounded(px(0.0))
                            .bg(if is_active || is_renaming_thread {
                                selected_bg
                            } else {
                                transparent
                            })
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |view, event: &MouseDownEvent, _window, cx| {
                                    if event.click_count >= 2 {
                                        view.begin_rename_agent_thread(thread_id.as_str(), cx);
                                    } else {
                                        if let Some(tab_index) = linked_tab_id
                                            .and_then(|tab_id| view.tab_index_by_id(tab_id))
                                        {
                                            view.switch_tab(tab_index, cx);
                                        } else if let Err(error) =
                                            view.resume_saved_agent_thread(thread_id.as_str(), cx)
                                        {
                                            termy_toast::error(error);
                                            view.notify_overlay(cx);
                                        }
                                        if view.renaming_agent_thread_id.as_deref()
                                            != Some(thread_id.as_str())
                                        {
                                            view.cancel_rename_agent_thread(cx);
                                        }
                                    }
                                    cx.stop_propagation();
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |view, _event, _window, cx| {
                                    view.schedule_agent_thread_context_menu(
                                        thread_context_menu_id.clone(),
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .justify_between()
                                    .gap(px(10.0))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .flex()
                                            .gap(px(8.0))
                                            .child(Self::render_agent_sidebar_avatar(
                                                thread.agent,
                                                dark_surface,
                                                border,
                                                input_bg,
                                                muted,
                                            ))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w(px(0.0))
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(3.0))
                                                    .child(div().relative().h(px(18.0)).child(
                                                        if is_renaming_thread {
                                                            self.render_inline_input_layer(
                                                                Font {
                                                                    family: self
                                                                        .font_family
                                                                        .clone(),
                                                                    weight: FontWeight::NORMAL,
                                                                    ..Default::default()
                                                                },
                                                                px(14.0),
                                                                text.into(),
                                                                selected_bg.into(),
                                                                InlineInputAlignment::Left,
                                                                cx,
                                                            )
                                                        } else {
                                                            div()
                                                                .truncate()
                                                                .text_size(px(14.5))
                                                                .text_color(text)
                                                                .child(title)
                                                                .into_any_element()
                                                        },
                                                    ))
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap(px(6.0))
                                                            .child(Self::render_agent_status_badge(
                                                                status.label.as_str(),
                                                                status.tone,
                                                                border,
                                                                input_bg,
                                                                text,
                                                                muted,
                                                                self.colors.ansi[11],
                                                                self.colors.ansi[9],
                                                            ))
                                                            .children(detail.map(|detail| {
                                                                div()
                                                                    .flex_1()
                                                                    .min_w(px(0.0))
                                                                    .truncate()
                                                                    .text_size(px(10.5))
                                                                    .text_color(muted)
                                                                    .child(detail)
                                                            })),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .pt(px(2.0))
                                            .text_size(px(11.0))
                                            .text_color(muted)
                                            .child(age),
                                    ),
                            )
                            .into_any_element()
                    })
                    .collect::<Vec<_>>();

                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(project_row)
                    .children(thread_rows)
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let empty_state = project_groups.is_empty().then(|| {
            div()
                .px(px(14.0))
                .py(px(12.0))
                .text_size(px(12.0))
                .text_color(muted)
                .child("No threads yet. Start an agent to create a project.")
                .into_any_element()
        });

        Some(
            div()
                .id("agent-sidebar")
                .w(px(AGENT_SIDEBAR_WIDTH))
                .h_full()
                .flex_none()
                .flex()
                .flex_col()
                .bg(panel_bg)
                .border_r_1()
                .border_color(border)
                .child(
                    div()
                        .h(px(AGENT_SIDEBAR_HEADER_HEIGHT))
                        .px(px(14.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(15.0))
                                .text_color(muted)
                                .child("Threads"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .id("agent-sidebar-new-thread")
                                        .w(px(20.0))
                                        .h(px(18.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(move |style| style.bg(button_hover_bg))
                                        .child(Self::render_agent_sidebar_new_session_icon(
                                            muted, panel_bg,
                                        ))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|view, _event, _window, cx| {
                                                view.open_command_palette_in_mode(
                                                    command_palette::CommandPaletteMode::AgentProjects,
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            }),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("agent-sidebar-hide")
                                        .w(px(20.0))
                                        .h(px(18.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(move |style| style.bg(button_hover_bg))
                                        .child(Self::render_agent_sidebar_hide_icon(muted))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|view, _event, _window, cx| {
                                                view.agent_sidebar_open = false;
                                                view.sync_persisted_agent_workspace();
                                            cx.notify();
                                            cx.stop_propagation();
                                        }),
                                    ),
                                ),
                        ),
                )
                .child(
                    div()
                        .id("agent-sidebar-scroll")
                        .flex_1()
                        .overflow_y_scroll()
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .flex_col()
                                .children(project_groups)
                                .children(empty_state),
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn pi_status_detects_setup_when_no_model_is_available() {
        let status = TerminalView::detect_provider_status(
            command_palette::AiAgentPreset::Pi,
            &lines(&["/private/tmp", "0.0%/0 (auto) no-model"]),
        )
        .expect("status");

        assert_eq!(status.label, "setup");
        assert_eq!(status.detail.as_deref(), Some("0.0%/0 (auto) no-model"));
        assert_eq!(status.tone, AgentThreadStatusTone::Warning);
    }

    #[test]
    fn opencode_status_detects_ready_from_prompt_footer() {
        let status = TerminalView::detect_provider_status(
            command_palette::AiAgentPreset::OpenCode,
            &lines(&[
                "Ask anything... What is the tech stack of this project?",
                "Build Big Pickle OpenCode Zen",
                "/private/tmp 1.3.1",
            ]),
        )
        .expect("status");

        assert_eq!(status.label, "ready");
        assert_eq!(
            status.detail.as_deref(),
            Some("Build Big Pickle OpenCode Zen")
        );
        assert_eq!(status.tone, AgentThreadStatusTone::Active);
    }

    #[test]
    fn claude_status_detects_connectivity_failure() {
        let status = TerminalView::detect_provider_status(
            command_palette::AiAgentPreset::Claude,
            &lines(&[
                "Unable to connect to Anthropic services",
                "Failed to connect to api.anthropic.com: ECONNREFUSED",
            ]),
        )
        .expect("status");

        assert_eq!(status.label, "error");
        assert_eq!(
            status.detail.as_deref(),
            Some("Failed to connect to api.anthropic.com: ECONNREFUSED")
        );
        assert_eq!(status.tone, AgentThreadStatusTone::Error);
    }
}
