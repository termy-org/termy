use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentProvider {
    Gemini,
    Codex,
    ClaudeCode,
}

impl AgentProvider {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Gemini => "Gemini",
            Self::Codex => "Codex",
            Self::ClaudeCode => "Claude Code",
        }
    }
}

impl From<termy_config_core::AiProvider> for AgentProvider {
    fn from(value: termy_config_core::AiProvider) -> Self {
        match value {
            termy_config_core::AiProvider::Gemini => Self::Gemini,
            termy_config_core::AiProvider::Codex => Self::Codex,
            termy_config_core::AiProvider::ClaudeCode => Self::ClaudeCode,
        }
    }
}

impl From<AgentProvider> for termy_config_core::AiProvider {
    fn from(value: AgentProvider) -> Self {
        match value {
            AgentProvider::Gemini => Self::Gemini,
            AgentProvider::Codex => Self::Codex,
            AgentProvider::ClaudeCode => Self::ClaudeCode,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMessageRole {
    User,
    Assistant,
    Tool,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMessage {
    pub role: AgentMessageRole,
    pub content: String,
    pub streaming: bool,
    pub created_at: std::time::Instant,
}

impl AgentMessage {
    pub fn new(role: AgentMessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            streaming: false,
            created_at: std::time::Instant::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentSession {
    pub id: u64,
    pub cwd: String,
    pub provider: AgentProvider,
    pub model: String,
    pub title: String,
    pub running: bool,
    pub running_since: Option<std::time::Instant>,
    pub messages: Vec<AgentMessage>,
}

impl AgentSession {
    fn new(
        id: u64,
        cwd: impl Into<String>,
        provider: AgentProvider,
        model: impl Into<String>,
    ) -> Self {
        let cwd = cwd.into();
        let title = PathBuf::from(&cwd)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("session")
            .to_string();

        Self {
            id,
            cwd,
            provider,
            model: model.into(),
            title,
            running: false,
            running_since: None,
            messages: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct AgentSessionStore {
    sessions: Vec<AgentSession>,
    active_index: Option<usize>,
    next_id: u64,
}

impl AgentSessionStore {
    pub fn ensure_session(
        &mut self,
        cwd: impl Into<String>,
        provider: AgentProvider,
        model: impl Into<String>,
    ) -> u64 {
        let cwd = cwd.into();
        if let Some(index) = self.sessions.iter().position(|session| session.cwd == cwd) {
            self.active_index = Some(index);
            return self.sessions[index].id;
        }

        self.new_session(cwd, provider, model)
    }

    pub fn new_session(
        &mut self,
        cwd: impl Into<String>,
        provider: AgentProvider,
        model: impl Into<String>,
    ) -> u64 {
        let next_id = self.next_id.saturating_add(1);
        self.next_id = next_id;
        let session = AgentSession::new(next_id, cwd, provider, model);
        self.sessions.push(session);
        self.active_index = Some(self.sessions.len().saturating_sub(1));
        next_id
    }

    pub fn set_active_by_id(&mut self, id: u64) -> bool {
        if let Some(index) = self.sessions.iter().position(|session| session.id == id) {
            self.active_index = Some(index);
            true
        } else {
            false
        }
    }

    pub fn sessions(&self) -> &[AgentSession] {
        &self.sessions
    }

    pub fn active_session(&self) -> Option<&AgentSession> {
        self.active_index.and_then(|index| self.sessions.get(index))
    }

    pub fn active_session_id(&self) -> Option<u64> {
        self.active_session().map(|session| session.id)
    }

    pub fn active_session_mut(&mut self) -> Option<&mut AgentSession> {
        self.active_index
            .and_then(|index| self.sessions.get_mut(index))
    }

    pub fn set_active_provider(&mut self, provider: AgentProvider) {
        if let Some(session) = self.active_session_mut() {
            session.provider = provider;
        }
    }

    pub fn set_active_model(&mut self, model: impl Into<String>) {
        if let Some(session) = self.active_session_mut() {
            session.model = model.into();
        }
    }

    pub fn set_active_running(&mut self, running: bool) {
        if let Some(session) = self.active_session_mut() {
            session.running = running;
            session.running_since = if running {
                Some(std::time::Instant::now())
            } else {
                None
            };
        }
    }

    pub fn push_active_message(&mut self, role: AgentMessageRole, content: impl Into<String>) {
        if let Some(session) = self.active_session_mut() {
            session.messages.push(AgentMessage::new(role, content));
        }
    }

    pub fn start_active_assistant_stream(&mut self) {
        if let Some(session) = self.active_session_mut() {
            session.messages.push(AgentMessage {
                role: AgentMessageRole::Assistant,
                content: String::new(),
                streaming: true,
                created_at: std::time::Instant::now(),
            });
        }
    }

    pub fn push_active_assistant_chunk(&mut self, chunk: &str) {
        if let Some(session) = self.active_session_mut()
            && let Some(message) =
                session.messages.iter_mut().rev().find(|message| {
                    message.streaming && message.role == AgentMessageRole::Assistant
                })
        {
            message.content.push_str(chunk);
        }
    }

    pub fn replace_active_assistant_stream_content(&mut self, content: impl Into<String>) {
        if let Some(session) = self.active_session_mut()
            && let Some(message) =
                session.messages.iter_mut().rev().find(|message| {
                    message.streaming && message.role == AgentMessageRole::Assistant
                })
        {
            message.content = content.into();
        }
    }

    pub fn finish_active_assistant_stream(&mut self) {
        if let Some(session) = self.active_session_mut()
            && let Some(message) =
                session.messages.iter_mut().rev().find(|message| {
                    message.streaming && message.role == AgentMessageRole::Assistant
                })
        {
            message.streaming = false;
        }
    }

    pub fn active_history_for_model(&self) -> Vec<(String, String)> {
        self.active_session()
            .map(|session| {
                session
                    .messages
                    .iter()
                    .map(|message| {
                        let role = match message.role {
                            AgentMessageRole::User => "user",
                            AgentMessageRole::Assistant => "assistant",
                            AgentMessageRole::Tool | AgentMessageRole::Error => "user",
                        };
                        (role.to_string(), message.content.clone())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
