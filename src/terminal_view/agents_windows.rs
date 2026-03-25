use super::*;

const AGENT_SIDEBAR_MIN_WIDTH: f32 = 180.0;
const AGENT_SIDEBAR_MAX_WIDTH: f32 = 500.0;
const AGENT_SIDEBAR_UNAVAILABLE_MESSAGE: &str =
    "Agent sidebar is currently unavailable on Windows builds.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AgentProject {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) root_path: String,
    pub(super) created_at_ms: u64,
    pub(super) updated_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AgentThread;

pub(super) fn clamp_agent_sidebar_width(width: f32) -> f32 {
    width.clamp(AGENT_SIDEBAR_MIN_WIDTH, AGENT_SIDEBAR_MAX_WIDTH)
}

impl TerminalView {
    pub(in super::super) fn agent_sidebar_width(&self) -> f32 {
        0.0
    }

    pub(in super::super) fn terminal_left_sidebar_width(&self) -> f32 {
        self.tab_strip_sidebar_width()
    }

    pub(super) fn should_render_agent_sidebar(&self) -> bool {
        false
    }

    pub(super) fn restore_persisted_agent_workspace(&mut self) {
        self.agent_sidebar_enabled = false;
        self.agent_sidebar_open = false;
        self.active_agent_project_id = None;
        self.collapsed_agent_project_ids.clear();
        self.agent_projects.clear();
        self.agent_threads.clear();
        self.renaming_agent_thread_id = None;
        self.agent_thread_rename_input.clear();
        self.agent_sidebar_search_active = false;
        self.agent_sidebar_search_input.clear();
    }

    pub(super) fn sync_persisted_agent_workspace(&self) {}

    pub(super) fn toggle_agent_sidebar(&mut self, cx: &mut Context<Self>) {
        termy_toast::info(AGENT_SIDEBAR_UNAVAILABLE_MESSAGE);
        self.notify_overlay(cx);
    }

    pub(super) fn normalized_agent_working_dir(
        &mut self,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        resolve_launch_working_directory(
            self.configured_working_dir.as_deref(),
            self.terminal_runtime.working_dir_fallback,
        )
        .map(|path| path.to_string_lossy().into_owned())
        .or_else(|| Self::user_home_dir().map(|path| path.to_string_lossy().into_owned()))
    }

    pub(super) fn launch_ai_agent_from_palette(
        &mut self,
        _agent: command_palette::AiAgentPreset,
        _project_id: Option<&str>,
        _cx: &mut Context<Self>,
    ) -> Result<(), String> {
        Err(AGENT_SIDEBAR_UNAVAILABLE_MESSAGE.to_string())
    }

    pub(super) fn delete_agent_thread(&mut self, _thread_id: &str) -> Result<(), String> {
        Err(AGENT_SIDEBAR_UNAVAILABLE_MESSAGE.to_string())
    }

    pub(super) fn delete_agent_project(&mut self, _project_id: &str) -> Result<usize, String> {
        Err(AGENT_SIDEBAR_UNAVAILABLE_MESSAGE.to_string())
    }

    pub(super) fn agent_thread_archive_snapshot_for_tab(
        &self,
        _index: usize,
    ) -> Option<(
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> {
        None
    }

    pub(super) fn archive_agent_thread_snapshot(
        &mut self,
        _thread_id: Option<&str>,
        _title: &str,
        _current_command: Option<&str>,
        _status_label: Option<&str>,
        _status_detail: Option<&str>,
    ) {
    }

    pub(super) fn sync_agent_workspace_to_active_tab(&mut self) {}

    pub(super) fn dismiss_agent_sidebar_search(&mut self, cx: &mut Context<Self>) {
        let had_query = !self.agent_sidebar_search_input.text().is_empty();
        let was_active = self.agent_sidebar_search_active;
        if !had_query && !was_active {
            return;
        }

        self.agent_sidebar_search_active = false;
        self.agent_sidebar_search_input.clear();
        self.inline_input_selecting = false;
        cx.notify();
    }

    pub(super) fn begin_rename_agent_thread(&mut self, _thread_id: &str, _cx: &mut Context<Self>) {}

    pub(super) fn commit_rename_agent_thread(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn open_ai_agents_palette_for_project_from_sidebar(
        &mut self,
        _project_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        termy_toast::info(AGENT_SIDEBAR_UNAVAILABLE_MESSAGE);
        self.notify_overlay(cx);
    }

    pub(super) fn open_first_matching_agent_thread(&mut self, _cx: &mut Context<Self>) {}

    pub(super) fn render_agent_sidebar(&mut self, _cx: &mut Context<Self>) -> Option<AnyElement> {
        None
    }
}
