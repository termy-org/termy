use super::*;

#[derive(Clone, Copy)]
pub(crate) struct GitPanelTheme {
    pub(crate) panel_bg: crate::gpui::Rgba,
    pub(crate) input_bg: crate::gpui::Rgba,
    pub(crate) selected_bg: crate::gpui::Rgba,
    pub(crate) border: crate::gpui::Rgba,
    pub(crate) text: crate::gpui::Rgba,
    pub(crate) muted: crate::gpui::Rgba,
    pub(crate) success: crate::gpui::Rgba,
    pub(crate) warning: crate::gpui::Rgba,
    pub(crate) danger: crate::gpui::Rgba,
    pub(crate) info: crate::gpui::Rgba,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AgentGitPanelState {
    pub(crate) open: bool,
    pub(crate) source_path: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) repo_root: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) current_branch: Option<String>,
    pub(crate) ahead: usize,
    pub(crate) behind: usize,
    pub(crate) dirty_count: usize,
    pub(crate) last_commit: Option<String>,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) filter: AgentGitPanelFilter,
    pub(crate) entries: Vec<AgentGitPanelEntry>,
    pub(crate) selected_repo_path: Option<String>,
    pub(crate) preview_loading: bool,
    pub(crate) preview_error: Option<String>,
    pub(crate) preview_diff_lines: Vec<String>,
    pub(crate) preview_history: Vec<AgentGitHistoryEntry>,
    pub(crate) project_history: Vec<AgentGitHistoryEntry>,
    pub(crate) branches: Vec<String>,
    pub(crate) stashes: Vec<AgentGitStashEntry>,
    pub(crate) just_committed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentGitPanelEntry {
    pub(crate) status: String,
    pub(crate) path: String,
    pub(crate) repo_path: String,
}

pub(crate) struct AgentGitPanelSnapshot {
    pub(crate) repo_root: String,
    pub(crate) branch: Option<String>,
    pub(crate) current_branch: Option<String>,
    pub(crate) ahead: usize,
    pub(crate) behind: usize,
    pub(crate) dirty_count: usize,
    pub(crate) last_commit: Option<String>,
    pub(crate) entries: Vec<AgentGitPanelEntry>,
    pub(crate) project_history: Vec<AgentGitHistoryEntry>,
    pub(crate) branches: Vec<String>,
    pub(crate) stashes: Vec<AgentGitStashEntry>,
}

pub(crate) struct AgentGitPanelPreviewSnapshot {
    pub(crate) diff_lines: Vec<String>,
    pub(crate) history: Vec<AgentGitHistoryEntry>,
}

impl AgentGitPanelEntry {
    pub(crate) fn from_status_line(status: &str, raw_path: &str) -> Self {
        let repo_path = raw_path
            .split(" -> ")
            .last()
            .unwrap_or(raw_path)
            .to_string();
        Self {
            status: status.to_string(),
            path: raw_path.to_string(),
            repo_path,
        }
    }

    pub(crate) fn is_untracked(&self) -> bool {
        self.status == "??"
    }

    pub(crate) fn is_staged(&self) -> bool {
        self.status
            .chars()
            .next()
            .is_some_and(|ch| ch != ' ' && ch != '?')
    }

    pub(crate) fn is_unstaged(&self) -> bool {
        self.status
            .chars()
            .nth(1)
            .is_some_and(|ch| ch != ' ' && ch != '?')
    }

    pub(crate) fn is_deleted(&self) -> bool {
        self.status.contains('D')
    }

    pub(crate) fn badge_label(&self) -> SharedString {
        if self.is_untracked() {
            return "new".into();
        }
        if self.status.contains('R') {
            return "ren".into();
        }
        if self.status.contains('A') {
            return "add".into();
        }
        if self.status.contains('D') {
            return "del".into();
        }
        if self.status.contains('U') {
            return "conf".into();
        }
        if self.status.contains('M') {
            return "mod".into();
        }
        self.status.trim().to_lowercase().into()
    }

    pub(crate) fn status_color(&self, theme: &GitPanelTheme) -> crate::gpui::Rgba {
        if self.is_untracked() || self.status.contains('A') {
            theme.success
        } else if self.status.contains('D') || self.status.contains('U') {
            theme.danger
        } else if self.status.contains('R') {
            theme.info
        } else {
            theme.warning
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AgentGitPanelFilter {
    #[default]
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentGitPanelInputMode {
    Commit,
    CreateBranch,
    SaveStash,
}

impl AgentGitPanelInputMode {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Commit => "Commit message",
            Self::CreateBranch => "New branch",
            Self::SaveStash => "Stash message",
        }
    }

    pub(crate) fn placeholder(self) -> &'static str {
        match self {
            Self::Commit => "Write a commit message",
            Self::CreateBranch => "feature/my-branch",
            Self::SaveStash => "WIP stash",
        }
    }

    pub(crate) fn action_label(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::CreateBranch => "create",
            Self::SaveStash => "save",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentGitHistoryEntry {
    pub(crate) summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentGitStashEntry {
    pub(crate) name: String,
    pub(crate) summary: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentThreadRuntimeStatus {
    Busy,
    Ready,
    Saved,
}

impl AgentThreadRuntimeStatus {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Ready => "ready",
            Self::Saved => "saved",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentThreadStatusTone {
    Active,
    Warning,
    Error,
    Muted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentThreadStatusPresentation {
    pub(crate) label: String,
    pub(crate) detail: Option<String>,
    pub(crate) tone: AgentThreadStatusTone,
}

pub(crate) struct AgentSidebarTooltip {
    title: &'static str,
    detail: &'static str,
    bg: crate::gpui::Rgba,
    border: crate::gpui::Rgba,
    text: crate::gpui::Rgba,
    muted: crate::gpui::Rgba,
}

impl AgentSidebarTooltip {
    pub(crate) fn new(
        title: &'static str,
        detail: &'static str,
        bg: crate::gpui::Rgba,
        border: crate::gpui::Rgba,
        text: crate::gpui::Rgba,
        muted: crate::gpui::Rgba,
    ) -> Self {
        Self {
            title,
            detail,
            bg,
            border,
            text,
            muted,
        }
    }
}

impl Render for AgentSidebarTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().pl(px(8.0)).pt(px(10.0)).child(
            div()
                .w(px(196.0))
                .px(px(10.0))
                .py(px(8.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .bg(self.bg)
                .border_1()
                .border_color(self.border)
                .child(
                    div()
                        .w_full()
                        .whitespace_normal()
                        .text_size(px(11.0))
                        .text_color(self.text)
                        .child(self.title),
                )
                .child(
                    div()
                        .w_full()
                        .whitespace_normal()
                        .text_size(px(10.0))
                        .text_color(self.muted)
                        .child(self.detail),
                ),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PersistedAgentWorkspaceState {
    pub(crate) version: u64,
    pub(crate) sidebar_open: bool,
    pub(crate) active_project_id: Option<String>,
    #[serde(default)]
    pub(crate) collapsed_project_ids: Vec<String>,
    pub(crate) projects: Vec<AgentProject>,
    pub(crate) threads: Vec<AgentThread>,
}

impl Default for PersistedAgentWorkspaceState {
    fn default() -> Self {
        Self {
            version: AGENT_WORKSPACE_SCHEMA_VERSION,
            sidebar_open: false,
            active_project_id: None,
            collapsed_project_ids: Vec::new(),
            projects: Vec::new(),
            threads: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AgentProject {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) root_path: String,
    #[serde(default)]
    pub(crate) pinned: bool,
    pub(crate) created_at_ms: u64,
    pub(crate) updated_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AgentThread {
    pub(crate) id: String,
    pub(crate) project_id: String,
    pub(crate) agent: command_palette::AiAgentPreset,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) custom_title: Option<String>,
    #[serde(default)]
    pub(crate) pinned: bool,
    pub(crate) launch_command: String,
    pub(crate) working_dir: String,
    pub(crate) last_seen_title: Option<String>,
    pub(crate) last_seen_command: Option<String>,
    #[serde(default)]
    pub(crate) last_status_label: Option<String>,
    #[serde(default)]
    pub(crate) last_status_detail: Option<String>,
    #[serde(default)]
    pub(crate) last_session_id: Option<String>,
    pub(crate) created_at_ms: u64,
    pub(crate) updated_at_ms: u64,
    #[serde(skip)]
    pub(crate) linked_tab_id: Option<TabId>,
}
