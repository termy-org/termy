use serde::Serialize;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use thiserror::Error;

pub const DEFAULT_MODEL: &str = "gpt-5.3-codex";

#[derive(Error, Debug)]
pub enum CodexError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("app-server response error: {message}")]
    ApiError { message: String },

    #[error("No response content")]
    NoContent,

    #[error("Unexpected app-server EOF")]
    UnexpectedEof,

    #[error("Shared Codex session is unavailable")]
    SessionUnavailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: ChatContent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ChatContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: ChatContent::Text(content.into()),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: ChatContent::Text(content.into()),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: ChatContent::Text(content.into()),
        }
    }

    pub fn user_with_file(text: impl Into<String>, file_content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: ChatContent::Parts(vec![ContentPart::Text {
                text: format!(
                    "{}\n\nTerminal context:\n```\n{}\n```",
                    text.into(),
                    file_content.into()
                ),
            }]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexModel {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct CodexClient {
    api_key: String,
    model: String,
    reasoning_effort: Option<String>,
}

impl CodexClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: DEFAULT_MODEL.to_string(),
            reasoning_effort: None,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_reasoning_effort(mut self, reasoning_effort: Option<String>) -> Self {
        self.reasoning_effort = reasoning_effort;
        self
    }

    pub fn chat(&self, messages: Vec<ChatMessage>) -> Result<String, CodexError> {
        self.chat_stream(messages, |_| {})
    }

    pub fn chat_stream<F>(
        &self,
        messages: Vec<ChatMessage>,
        on_chunk: F,
    ) -> Result<String, CodexError>
    where
        F: FnMut(&str),
    {
        self.chat_stream_with_tool_updates(messages, on_chunk, |_| {})
    }

    pub fn chat_stream_with_tool_updates<F, G>(
        &self,
        messages: Vec<ChatMessage>,
        mut on_chunk: F,
        mut on_tool_update: G,
    ) -> Result<String, CodexError>
    where
        F: FnMut(&str),
        G: FnMut(&str),
    {
        let cwd = std::env::current_dir()
            .ok()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| ".".to_string());
        let input = render_messages_as_input(messages);
        let reasoning_effort = self.reasoning_effort.clone();
        let model = self.model.clone();

        self.with_session(|session| {
            let thread_id = session.start_thread(&cwd, &model)?;
            let turn_id = session.start_turn(&thread_id, &input, reasoning_effort.as_deref())?;

            let mut output = String::new();
            let mut completed_message: Option<String> = None;
            let mut terminal_error: Option<String> = None;

            loop {
                let message = session.read_message()?;
                let method = message
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let params = message.get("params").cloned().unwrap_or(Value::Null);

                match method {
                    "item/agentMessage/delta" => {
                        if params
                            .get("turnId")
                            .and_then(Value::as_str)
                            .is_some_and(|value| value == turn_id)
                            && let Some(delta) = params.get("delta").and_then(Value::as_str)
                        {
                            on_chunk(delta);
                            output.push_str(delta);
                        }
                    }
                    "item/started" => {
                        if params
                            .get("turnId")
                            .and_then(Value::as_str)
                            .is_some_and(|value| value == turn_id)
                            && let Some(item) = params.get("item")
                            && let Some(text) = format_tool_item_started(item)
                        {
                            on_tool_update(&text);
                        }
                    }
                    "item/completed" => {
                        if params
                            .get("turnId")
                            .and_then(Value::as_str)
                            .is_some_and(|value| value == turn_id)
                            && let Some(item) = params.get("item")
                        {
                            if item
                                .get("type")
                                .and_then(Value::as_str)
                                .is_some_and(|value| value == "agentMessage")
                                && let Some(text) = item.get("text").and_then(Value::as_str)
                            {
                                completed_message = Some(text.to_string());
                            } else if let Some(text) = format_tool_item_completed(item) {
                                on_tool_update(&text);
                            }
                        }
                    }
                    "item/commandExecution/outputDelta" => {
                        // Intentionally ignored in the UI stream.
                        // Command output can be very large and destabilizes sidebar layout;
                        // we surface concise started/completed tool status messages instead.
                    }
                    "error" => {
                        if params
                            .get("turnId")
                            .and_then(Value::as_str)
                            .is_some_and(|value| value == turn_id)
                        {
                            let will_retry = params
                                .get("willRetry")
                                .and_then(Value::as_bool)
                                .unwrap_or(false);
                            if !will_retry {
                                terminal_error = params
                                    .get("error")
                                    .and_then(|error| error.get("message"))
                                    .and_then(Value::as_str)
                                    .map(ToString::to_string);
                            }
                        }
                    }
                    "turn/completed" => {
                        let turn = params.get("turn").cloned().unwrap_or(Value::Null);
                        let completed_turn_id =
                            turn.get("id").and_then(Value::as_str).unwrap_or_default();
                        if completed_turn_id != turn_id {
                            continue;
                        }

                        let status = turn
                            .get("status")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if status == "failed" {
                            let message = turn
                                .get("error")
                                .and_then(|error| error.get("message"))
                                .and_then(Value::as_str)
                                .map(ToString::to_string)
                                .or(terminal_error)
                                .unwrap_or_else(|| "turn failed".to_string());
                            return Err(CodexError::ApiError { message });
                        }

                        if output.is_empty()
                            && let Some(text) = completed_message
                        {
                            output = text;
                        }
                        break;
                    }
                    _ => {}
                }
            }

            if output.trim().is_empty() {
                return Err(CodexError::NoContent);
            }
            Ok(output)
        })
    }

    pub fn message_with_terminal_context(
        &self,
        user_message: impl Into<String>,
        terminal_content: impl Into<String>,
    ) -> Result<String, CodexError> {
        let system = ChatMessage::system(
            "You are a helpful terminal assistant. The user will provide terminal context \
             (recent commands and output). Help them with their question. When suggesting \
             commands, be concise and provide only the command they should run. \
             If they ask for a command, respond with just the command, no explanation unless asked.",
        );
        let user = ChatMessage::user_with_file(user_message, terminal_content);

        self.chat(vec![system, user])
    }

    pub fn fetch_models(&self) -> Result<Vec<CodexModel>, CodexError> {
        self.with_session(|session| {
            let mut models = Vec::new();
            let mut cursor: Option<String> = None;
            loop {
                let mut params = json!({});
                if let Some(value) = cursor.as_ref() {
                    params["cursor"] = Value::String(value.clone());
                }
                let result = session.send_request("model/list", params)?;
                let data = result
                    .get("data")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for model in data {
                    let id = model
                        .get("model")
                        .and_then(Value::as_str)
                        .or_else(|| model.get("id").and_then(Value::as_str));
                    if let Some(id) = id {
                        models.push(CodexModel { id: id.to_string() });
                    }
                }
                cursor = result
                    .get("nextCursor")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                if cursor.is_none() {
                    break;
                }
            }

            models.sort_by(|left, right| left.id.cmp(&right.id));
            models.dedup_by(|left, right| left.id == right.id);
            Ok(models)
        })
    }

    pub fn fetch_chat_models(&self) -> Result<Vec<CodexModel>, CodexError> {
        self.fetch_models()
    }

    fn with_session<T, F>(&self, mut operation: F) -> Result<T, CodexError>
    where
        F: FnMut(&mut AppServerSession) -> Result<T, CodexError>,
    {
        let mut guard = shared_session()
            .lock()
            .map_err(|_| CodexError::SessionUnavailable)?;

        for attempt in 0..=1 {
            if guard.session.is_none() {
                let mut session = AppServerSession::new()?;
                session.initialize()?;
                guard.session = Some(session);
                guard.logged_in_api_key = None;
            }

            if !self.api_key.trim().is_empty()
                && guard.logged_in_api_key.as_deref() != Some(self.api_key.as_str())
                && let Some(session) = guard.session.as_mut()
            {
                session.login_if_needed(&self.api_key)?;
                guard.logged_in_api_key = Some(self.api_key.clone());
            }

            let Some(session) = guard.session.as_mut() else {
                return Err(CodexError::SessionUnavailable);
            };

            match operation(session) {
                Ok(value) => return Ok(value),
                Err(error) => {
                    if attempt == 0 && should_reset_session(&error) {
                        guard.session = None;
                        guard.logged_in_api_key = None;
                        continue;
                    }
                    return Err(error);
                }
            }
        }

        Err(CodexError::SessionUnavailable)
    }
}

fn format_tool_item_started(item: &Value) -> Option<String> {
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
    match item_type {
        "commandExecution" => {
            let command = item
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("<unknown>");
            Some(format!("Running command: `{command}`"))
        }
        "mcpToolCall" => {
            let server = item
                .get("server")
                .and_then(Value::as_str)
                .unwrap_or("<server>");
            let tool = item.get("tool").and_then(Value::as_str).unwrap_or("<tool>");
            Some(format!("Calling MCP tool: `{server}/{tool}`"))
        }
        "dynamicToolCall" => {
            let tool = item.get("tool").and_then(Value::as_str).unwrap_or("<tool>");
            Some(format!("Calling tool: `{tool}`"))
        }
        "fileChange" => Some("Applying file changes...".to_string()),
        "webSearch" => {
            let query = item.get("query").and_then(Value::as_str).unwrap_or("");
            Some(format!("Searching web: `{query}`"))
        }
        _ => None,
    }
}

fn format_tool_item_completed(item: &Value) -> Option<String> {
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
    match item_type {
        "commandExecution" => {
            let command = item
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("<unknown>");
            let status = item
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("completed");
            let exit_code = item
                .get("exitCode")
                .and_then(Value::as_i64)
                .map(|value| format!(" (exit {value})"))
                .unwrap_or_default();
            Some(format!("Command {status}{exit_code}: `{command}`"))
        }
        "mcpToolCall" => {
            let server = item
                .get("server")
                .and_then(Value::as_str)
                .unwrap_or("<server>");
            let tool = item.get("tool").and_then(Value::as_str).unwrap_or("<tool>");
            let status = item
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("completed");
            Some(format!("MCP tool {status}: `{server}/{tool}`"))
        }
        "dynamicToolCall" => {
            let tool = item.get("tool").and_then(Value::as_str).unwrap_or("<tool>");
            let status = item
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("completed");
            Some(format!("Tool {status}: `{tool}`"))
        }
        "fileChange" => {
            let status = item
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("completed");
            let count = item
                .get("changes")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            Some(format!("File changes {status}: {count} change(s)"))
        }
        "webSearch" => {
            let query = item.get("query").and_then(Value::as_str).unwrap_or("");
            Some(format!("Web search completed: `{query}`"))
        }
        _ => None,
    }
}

fn should_reset_session(error: &CodexError) -> bool {
    match error {
        CodexError::UnexpectedEof => true,
        CodexError::IoError(source) => matches!(
            source.kind(),
            std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::NotConnected
        ),
        _ => false,
    }
}

fn render_messages_as_input(messages: Vec<ChatMessage>) -> String {
    let mut lines = Vec::with_capacity(messages.len() + 1);
    for message in messages {
        let role = message.role.to_ascii_uppercase();
        let content = match message.content {
            ChatContent::Text(text) => text,
            ChatContent::Parts(parts) => parts
                .into_iter()
                .map(|part| match part {
                    ContentPart::Text { text } => text,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        };
        lines.push(format!("{role}: {content}"));
    }
    lines.push("ASSISTANT:".to_string());
    lines.join("\n\n")
}

#[derive(Default)]
struct SharedAppServerState {
    session: Option<AppServerSession>,
    logged_in_api_key: Option<String>,
}

fn shared_session() -> &'static Mutex<SharedAppServerState> {
    static SHARED: OnceLock<Mutex<SharedAppServerState>> = OnceLock::new();
    SHARED.get_or_init(|| Mutex::new(SharedAppServerState::default()))
}

struct AppServerSession {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl AppServerSession {
    fn new() -> Result<Self, CodexError> {
        let mut child = Command::new("codex")
            .arg("app-server")
            .arg("--listen")
            .arg("stdio://")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child.stdin.take().ok_or(CodexError::UnexpectedEof)?;
        let stdout = child.stdout.take().ok_or(CodexError::UnexpectedEof)?;

        Ok(Self {
            child,
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            next_id: 1,
        })
    }

    fn initialize(&mut self) -> Result<(), CodexError> {
        let params = json!({
            "clientInfo": {
                "name": "termy",
                "version": "0.1.0",
            }
        });
        let _ = self.send_request("initialize", params)?;
        Ok(())
    }

    fn login_if_needed(&mut self, api_key: &str) -> Result<(), CodexError> {
        if api_key.trim().is_empty() {
            return Ok(());
        }

        let params = json!({
            "type": "apiKey",
            "apiKey": api_key,
        });
        let _ = self.send_request("account/login/start", params)?;
        Ok(())
    }

    fn start_thread(&mut self, cwd: &str, model: &str) -> Result<String, CodexError> {
        let params = json!({
            "cwd": cwd,
            "model": model,
            "modelProvider": "openai",
        });
        let result = self.send_request("thread/start", params)?;
        result
            .get("thread")
            .and_then(|thread| thread.get("id"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .ok_or(CodexError::NoContent)
    }

    fn start_turn(
        &mut self,
        thread_id: &str,
        input: &str,
        reasoning_effort: Option<&str>,
    ) -> Result<String, CodexError> {
        let mut params = json!({
            "threadId": thread_id,
            "input": [
                {
                    "type": "text",
                    "text": input,
                }
            ]
        });
        if let Some(value) = reasoning_effort.filter(|value| !value.trim().is_empty()) {
            params["effort"] = Value::String(value.trim().to_string());
        }
        let result = self.send_request("turn/start", params)?;
        result
            .get("turn")
            .and_then(|turn| turn.get("id"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .ok_or(CodexError::NoContent)
    }

    fn send_request(&mut self, method: &str, params: Value) -> Result<Value, CodexError> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        serde_json::to_writer(&mut self.stdin, &request)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;

        loop {
            let message = self.read_message()?;
            let Some(message_id) = message.get("id") else {
                continue;
            };
            if !request_id_matches(message_id, id) {
                continue;
            }

            if let Some(error) = message.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("app-server request failed")
                    .to_string();
                return Err(CodexError::ApiError { message });
            }

            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    fn read_message(&mut self) -> Result<Value, CodexError> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.stdout.read_line(&mut line)? == 0 {
                return Err(CodexError::UnexpectedEof);
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(trimmed) {
                Ok(value) => return Ok(value),
                Err(_) => continue,
            }
        }
    }
}

impl Drop for AppServerSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn request_id_matches(value: &Value, expected: u64) -> bool {
    match value {
        Value::Number(number) => number.as_u64().is_some_and(|value| value == expected),
        Value::String(text) => text == expected.to_string().as_str(),
        _ => false,
    }
}
