use claude_code_sdk::{ClaudeCodeOptions, ContentBlock, Message, query};
use thiserror::Error;
use tokio::runtime::Builder;
use tokio_stream::StreamExt;

pub const DEFAULT_MODEL: &str = "sonnet";

#[derive(Error, Debug)]
pub enum ClaudeCodeError {
    #[error("Failed to initialize runtime: {0}")]
    RuntimeInit(#[from] std::io::Error),

    #[error("Claude Code request failed: {0}")]
    Query(String),

    #[error("No response content")]
    NoContent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeCodeModel {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct ClaudeCodeClient {
    model: String,
}

impl ClaudeCodeClient {
    pub fn new() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn chat(&self, prompt: impl Into<String>) -> Result<String, ClaudeCodeError> {
        let prompt = prompt.into();
        let runtime = Builder::new_current_thread().enable_all().build()?;
        runtime.block_on(async move {
            let mut stream = query(&prompt, Some(ClaudeCodeOptions::default()))
                .await
                .map_err(|error| ClaudeCodeError::Query(error.to_string()))?;
            let mut text_output = String::new();
            let mut result_output: Option<String> = None;

            while let Some(message) = stream.next().await {
                match message {
                    Message::Assistant(assistant_message) => {
                        for block in assistant_message.content {
                            if let ContentBlock::Text(text_block) = block {
                                text_output.push_str(&text_block.text);
                            }
                        }
                    }
                    Message::Result(result_message) => {
                        if let Some(result) = result_message.result
                            && !result.trim().is_empty()
                        {
                            result_output = Some(result);
                        }
                    }
                    _ => {}
                }
            }

            if !text_output.trim().is_empty() {
                return Ok(text_output);
            }
            if let Some(result) = result_output
                && !result.trim().is_empty()
            {
                return Ok(result);
            }
            Err(ClaudeCodeError::NoContent)
        })
    }

    pub fn message_with_terminal_context(
        &self,
        user_message: impl Into<String>,
        terminal_content: impl Into<String>,
    ) -> Result<String, ClaudeCodeError> {
        let prompt = format!(
            "You are a helpful terminal assistant. The user will provide terminal context \
             (recent commands and output). Help them with their question. When suggesting \
             commands, be concise and provide only the command they should run. \
             If they ask for a command, respond with just the command, no explanation unless asked.\n\n\
             User message:\n{}\n\n\
             Terminal context:\n```\n{}\n```",
            user_message.into(),
            terminal_content.into(),
        );
        self.chat(prompt)
    }

    pub fn fetch_chat_models(&self) -> Result<Vec<ClaudeCodeModel>, ClaudeCodeError> {
        let mut models = vec![
            ClaudeCodeModel {
                id: "sonnet".to_string(),
            },
            ClaudeCodeModel {
                id: "opus".to_string(),
            },
        ];
        let selected = self.model.trim();
        if !selected.is_empty() && !models.iter().any(|model| model.id == selected) {
            models.insert(
                0,
                ClaudeCodeModel {
                    id: selected.to_string(),
                },
            );
        }
        Ok(models)
    }
}

impl Default for ClaudeCodeClient {
    fn default() -> Self {
        Self::new()
    }
}
